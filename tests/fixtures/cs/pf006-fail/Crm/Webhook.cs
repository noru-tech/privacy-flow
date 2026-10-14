namespace Shop.Crm;

public class User
{
    public string Email { get; set; } = "";
}

public class Webhook(HttpClient http, IConfiguration config)
{
    public Task Notify(User user) =>
        http.PostAsJsonAsync(config["WebhookUrl"], new { email = user.Email }); // expect: PF006
}
