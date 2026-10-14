namespace Shop.Crm;

public class User
{
    public string Email { get; set; } = "";
}

public class Webhook(HttpClient http, IConfiguration config)
{
    public Task Notify(User user) =>
        http.PostAsJsonAsync("https://api.crm.acme.example/v2/contacts", new { email = user.Email });
}
