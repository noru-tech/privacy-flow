using Shop.Clients;

var builder = Host.CreateApplicationBuilder(args);
// The same client, registered against an outside host.
builder.Services.AddHttpClient<OrdersClient>(c => c.BaseAddress = new Uri("https://legacy.example.com"));
builder.Build().Run();
