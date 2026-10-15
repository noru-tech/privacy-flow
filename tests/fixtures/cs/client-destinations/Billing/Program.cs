var builder = WebApplication.CreateBuilder(args);
var app = builder.Build();

app.MapPost("/charge", () => Results.Ok());

app.Run();
