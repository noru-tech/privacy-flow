namespace Shop.Clients;

public record Customer(string Email, string Name);

// A typed client: its base address is set where it is registered, by each service.
public class OrdersClient(HttpClient http)
{
    public Task Send(Customer customer) =>
        http.PostAsJsonAsync("/orders", customer); // expect: PF006
}
