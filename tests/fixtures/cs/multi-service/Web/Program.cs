using Shop.Clients;

var builder = WebApplication.CreateBuilder(args);
builder.Services.AddHttpClient<OrdersClient>(c => c.BaseAddress = new("https+http://orders-api"));
var app = builder.Build();

app.MapPost("/checkout", (Customer customer, OrdersClient orders) => orders.Send(customer));

app.Run();
