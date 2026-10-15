var builder = DistributedApplication.CreateBuilder(args);

var billing = builder.AddProject("billing", "../Billing/Billing.csproj");
// The shop reaches billing by service discovery (WithReference) and by an environment URL.
builder.AddProject("shop", "../Shop/Shop.csproj")
    .WithReference(billing)
    .WithEnvironment("Billing__Url", billing.GetEndpoint("https"));

builder.Build().Run();
