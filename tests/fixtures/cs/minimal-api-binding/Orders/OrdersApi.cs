using Microsoft.AspNetCore.Mvc;
using Microsoft.EntityFrameworkCore;

namespace Shop.Orders;

public record CreateOrder(string Email, string CardNumber, int Quantity);

public interface IOrderStore
{
    Task Save(CreateOrder order);
}

public class ShopDb : DbContext
{
}

public class OrderStore(ShopDb db) : IOrderStore
{
    public Task Save(CreateOrder order)
    {
        db.Add(order);
        return db.SaveChangesAsync();
    }
}

public class OrderServices(ILogger<OrderServices> logger, IOrderStore store)
{
    public ILogger<OrderServices> Logger { get; } = logger;
    public IOrderStore Store { get; } = store;
}

public static class OrdersApi
{
    public static void MapOrders(this IEndpointRouteBuilder app)
    {
        app.MapPost("/orders", Create);
        app.MapGet("/orders/{id}", Get);
        app.MapGet("/search", Search);
    }

    // Services, the database context and the cancellation token are injected, not input.
    static async Task<IResult> Create(CreateOrder order, [AsParameters] OrderServices services, ShopDb db, CancellationToken ct)
    {
        services.Logger.LogInformation("order {@Order}", order); // expect: PF001
        await services.Store.Save(order);
        return Results.Ok();
    }

    // GUIDs are input only when their names say what they are.
    static IResult Get(Guid id, [FromHeader(Name = "x-request-id")] Guid requestId, ILogger<OrderServices> logger)
    {
        logger.LogInformation("get {Id} {RequestId}", id, requestId);
        return Results.Ok();
    }

    static IResult Search(string q, HttpContext context, ILogger<OrderServices> logger)
    {
        var referrer = context.Request.Headers["Referer"];
        logger.LogInformation("search {Q} from {Referrer}", q, referrer); // expect: PF001
        return Results.Ok();
    }
}
