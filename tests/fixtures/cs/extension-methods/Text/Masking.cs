namespace Shop.Text;

public static class Masking
{
    public static string Masked(this string value) => "***";

    public static string Shout(this string value) => value.ToUpperInvariant();
}

public class Greeter(ILogger<Greeter> logger)
{
    public void Greet(string email)
    {
        logger.LogInformation("hi {Email}", email.Masked());
        logger.LogInformation("HI {Email}", email.Shout()); // expect: PF001
    }
}
