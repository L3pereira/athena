use crate::application::ports::{EventPublisher, OrderBookRepository, RateLimiter};
use crate::domain::{
    Clock, ExchangeEvent, Order, OrderCanceledEvent, OrderId, OrderValidator, Symbol,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CancelOrderCommand {
    pub symbol: String,
    pub order_id: Option<OrderId>,
    pub client_order_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CancelOrderResult {
    pub order: Order,
}

pub struct CancelOrderUseCase<C, OB, E, R>
where
    C: Clock,
    OB: OrderBookRepository,
    E: EventPublisher,
    R: RateLimiter,
{
    clock: Arc<C>,
    order_book_repo: Arc<OB>,
    event_publisher: Arc<E>,
    rate_limiter: Arc<R>,
}

impl<C, OB, E, R> CancelOrderUseCase<C, OB, E, R>
where
    C: Clock,
    OB: OrderBookRepository,
    E: EventPublisher,
    R: RateLimiter,
{
    pub fn new(
        clock: Arc<C>,
        order_book_repo: Arc<OB>,
        event_publisher: Arc<E>,
        rate_limiter: Arc<R>,
    ) -> Self {
        Self {
            clock,
            order_book_repo,
            event_publisher,
            rate_limiter,
        }
    }

    pub async fn execute(
        &self,
        client_id: &str,
        command: CancelOrderCommand,
    ) -> Result<CancelOrderResult, CancelError> {
        // Check rate limit
        let rate_result = self.rate_limiter.check_request(client_id, 1).await;
        if !rate_result.allowed {
            return Err(CancelError::RateLimited {
                retry_after_ms: rate_result.retry_after.map(|d| d.as_millis() as u64),
            });
        }

        // Parse symbol
        let symbol =
            Symbol::new(&command.symbol).map_err(|e| CancelError::InvalidSymbol(e.to_string()))?;

        // Get order book
        let mut book = self
            .order_book_repo
            .get(&symbol)
            .await
            .ok_or_else(|| CancelError::SymbolNotFound(command.symbol.clone()))?;

        // Find the order
        let order_id = if let Some(id) = command.order_id {
            id
        } else if let Some(_client_id) = &command.client_order_id {
            // Would need to search by client order ID - not implemented for simplicity
            return Err(CancelError::OrderNotFound);
        } else {
            return Err(CancelError::MissingOrderId);
        };

        // Get order from book
        let order = book
            .get_order(order_id)
            .ok_or(CancelError::OrderNotFound)?
            .clone();

        // Validate cancellation
        OrderValidator::validate_cancel(&order)
            .map_err(|e| CancelError::ValidationFailed(e.message))?;

        // Remove from book
        let mut cancelled_order = book
            .remove_order(order_id)
            .ok_or(CancelError::OrderNotFound)?;

        // Update status
        let now = self.clock.now();
        cancelled_order.cancel(now);

        // Save book
        self.order_book_repo.save(book).await;

        // Publish event
        self.event_publisher
            .publish_to_symbol(
                symbol.as_str(),
                ExchangeEvent::OrderCanceled(OrderCanceledEvent {
                    order_id: cancelled_order.id,
                    client_order_id: cancelled_order.client_order_id.clone(),
                    symbol: cancelled_order.symbol.clone(),
                    timestamp: now,
                }),
            )
            .await;

        Ok(CancelOrderResult {
            order: cancelled_order,
        })
    }
}

#[derive(Debug, Clone)]
pub enum CancelError {
    RateLimited { retry_after_ms: Option<u64> },
    InvalidSymbol(String),
    SymbolNotFound(String),
    OrderNotFound,
    MissingOrderId,
    ValidationFailed(String),
}

impl std::fmt::Display for CancelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CancelError::RateLimited { retry_after_ms } => {
                write!(f, "Rate limited")?;
                if let Some(ms) = retry_after_ms {
                    write!(f, ", retry after {}ms", ms)?;
                }
                Ok(())
            }
            CancelError::InvalidSymbol(s) => write!(f, "Invalid symbol: {}", s),
            CancelError::SymbolNotFound(s) => write!(f, "Symbol not found: {}", s),
            CancelError::OrderNotFound => write!(f, "Order not found"),
            CancelError::MissingOrderId => {
                write!(f, "Either orderId or origClientOrderId must be provided")
            }
            CancelError::ValidationFailed(s) => write!(f, "Validation failed: {}", s),
        }
    }
}

impl std::error::Error for CancelError {}
