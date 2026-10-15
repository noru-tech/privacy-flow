var builder = DistributedApplication.CreateBuilder(args);

var orders = builder.AddProject<Projects.Orders_Api>("orders-api");
builder.AddProject("web", "../Web/Web.csproj").WithReference(orders);

builder.Build().Run();
