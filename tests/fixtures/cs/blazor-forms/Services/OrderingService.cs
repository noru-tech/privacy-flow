namespace Shop.Web.Services;

public class Address
{
    public string Street { get; set; } = "";
    public string City { get; set; } = "";
}

public class OrderingService(HttpClient http)
{
    public Task Place(Address address) => http.PostAsJsonAsync("api/orders", address); // expect: PF006
}
