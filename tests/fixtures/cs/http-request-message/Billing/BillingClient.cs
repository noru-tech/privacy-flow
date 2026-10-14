namespace Shop.Billing;

public class Customer
{
    public string Email { get; set; } = "";
}

public class BillingClient(HttpClient http, IHttpClientFactory factory)
{
    // The host is the literal the request was built with.
    public Task Sync(Customer customer)
    {
        var message = new HttpRequestMessage(HttpMethod.Post, "https://billing.example.com/customers")
        {
            Content = JsonContent.Create(new { email = customer.Email })
        };
        return http.SendAsync(message); // expect: PF002
    }

    // A relative URL resolves against the client's base address, set elsewhere.
    public Task Contact(Customer customer) =>
        factory.CreateClient("crm").PostAsJsonAsync("/contacts", customer); // expect: PF006
}
