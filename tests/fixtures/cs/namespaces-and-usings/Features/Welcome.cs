using Log = Microsoft.Extensions.Logging.ILogger;

namespace Shop.Features;

public class Welcome(Mailer mailer, Log log)
{
    public void Greet(string email)
    {
        mailer.Deliver(email, "Welcome!");
        log.LogInformation("greeted {Email}", email); // expect: PF001
    }
}
