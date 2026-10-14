namespace Shop.Accounts;

public class SignupService(ILogger<SignupService> logger)
{
    public void OnSignup(Account account)
    {
        logger.LogInformation("signup {Id}", account.Id);
        logger.LogInformation("welcome {Email}", Privacy.Mask(account.Email));
        logger.LogInformation("email length {Length}", account.Email.Length);
    }
}
