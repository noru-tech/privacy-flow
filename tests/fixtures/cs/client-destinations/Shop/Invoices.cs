namespace Shop;

public record Customer(string Email, string Name);

// A typed client: the HttpClient injected here is the one registered for Invoices.
public class Invoices(HttpClient http)
{
    public Task Send(Customer customer) =>
        // internal: the registration's base address names the billing service
        http.PostAsJsonAsync("/invoices", customer);
}
