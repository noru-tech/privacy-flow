using MediatR;

namespace Shop.Ordering;

public record PlaceOrder(string Email, string CardNumber) : IRequest<bool>;

public record CancelOrder(int OrderId) : IRequest<bool>;

public record OrderPlaced(string Email) : INotification;

public class PlaceOrderHandler(IMediator mediator) : IRequestHandler<PlaceOrder, bool>
{
    public async Task<bool> Handle(PlaceOrder request, CancellationToken ct)
    {
        await mediator.Publish(new OrderPlaced(request.Email), ct);
        return true;
    }
}

// Only cancellations reach this handler: what it logs holds no order data.
public class CancelOrderHandler(ILogger<CancelOrderHandler> logger) : IRequestHandler<CancelOrder, bool>
{
    public Task<bool> Handle(CancelOrder request, CancellationToken ct)
    {
        logger.LogInformation("cancel {@Request}", request);
        return Task.FromResult(true);
    }
}

// Every request passes through the pipeline behavior.
public class LoggingBehavior<TRequest, TResponse>(ILogger<LoggingBehavior<TRequest, TResponse>> logger)
    : IPipelineBehavior<TRequest, TResponse> where TRequest : notnull
{
    public async Task<TResponse> Handle(TRequest request, RequestHandlerDelegate<TResponse> next, CancellationToken ct)
    {
        logger.LogInformation("handling {@Request}", request); // expect: PF001
        return await next();
    }
}

public class WelcomeMailer(ILogger<WelcomeMailer> logger) : INotificationHandler<OrderPlaced>
{
    public Task Handle(OrderPlaced notification, CancellationToken ct)
    {
        logger.LogInformation("welcome {Email}", notification.Email); // expect: PF001
        return Task.CompletedTask;
    }
}

public class OrdersEndpoint(ISender sender)
{
    public Task<bool> Place(PlaceOrder order) => sender.Send(order);

    public Task<bool> Cancel(CancelOrder order) => sender.Send(order);
}
