using Shop;

var builder = WebApplication.CreateBuilder(args);
// A named client whose base address is the environment URL the app host sets.
builder.Services.AddHttpClient("billing", c => c.BaseAddress = new Uri(builder.Configuration["Billing:Url"]!));
// A typed client whose base address service discovery gives.
builder.Services.AddHttpClient<Invoices>(c =>
    c.BaseAddress = new(builder.Configuration.GetValue<string>("services:billing:https:0")!));
// A named client configured after it is added, against an outside host.
builder.Services.AddHttpClient("crm")
    .ConfigureHttpClient((sp, c) => c.BaseAddress = new Uri("https://api.crm.example"));
var app = builder.Build();

// Internal calls: the clients' base addresses name the billing service.
app.MapPost("/pay", (Customer c, IHttpClientFactory f) => f.CreateClient("billing").PostAsJsonAsync("/charge", c));
app.MapPost("/invoice", (Customer c, Invoices invoices) => invoices.Send(c));
app.MapPost("/lead", (Customer c, IHttpClientFactory f) => f.CreateClient("crm").PostAsJsonAsync("/leads", c)); // expect: PF002
// No registration names this client: its host is not in the code.
app.MapPost("/other", (Customer c, IHttpClientFactory f) => f.CreateClient("partner").PostAsJsonAsync("/x", c)); // expect: PF006

app.Run();
