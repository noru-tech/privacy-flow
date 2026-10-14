namespace Shop.Accounts;

public partial class SignupService(ILogger<SignupService> logger)
{
    public void OnSignup(Account account)
    {
        logger.LogInformation("signup {Id}", account.Id);
        logger.LogInformation("welcome {Email}", account.Email); // expect: PF001
        Console.WriteLine($"phone: {account.PhoneNumber}"); // expect: PF001
        LogWelcome(account.Email); // expect: PF001
    }

    [LoggerMessage(Level = LogLevel.Information, Message = "Welcome {Email}")]
    partial void LogWelcome(string email);
}
