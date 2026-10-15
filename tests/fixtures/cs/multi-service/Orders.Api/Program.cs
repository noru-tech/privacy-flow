using Shop.Orders.Data;
using Shop.Shared;

var builder = WebApplication.CreateBuilder(args);
var app = builder.Build();

app.MapPost("/orders", (CreateOrder order, ILogger<Program> log) =>
{
    log.LogInformation("order for {Email}", order.Email); // expect: PF001
    Audit.Note(log, order.Email);
    return Results.Ok();
});

app.Run();
