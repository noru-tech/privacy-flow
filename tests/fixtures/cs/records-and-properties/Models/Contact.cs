namespace Shop.Models;

public record Contact(string Name, string Email)
{
    public string Display => $"{Name} <{Email}>";
}

public class Envelope
{
    public string Subject { get; set; } = "";
    public Contact? To { get; init; }
}

public class Sender(ILogger<Sender> logger)
{
    public void Send(string name, string email)
    {
        var contact = new Contact(name, email);
        var envelope = new Envelope { Subject = "Hi", To = contact };
        logger.LogInformation("to {To}", envelope.To!.Email); // expect: PF001
        logger.LogInformation("subject {Subject}", envelope.Subject);
        var copy = contact with { Name = "anonymous" };
        logger.LogInformation("copy {Copy}", copy.Email); // expect: PF001
    }
}
