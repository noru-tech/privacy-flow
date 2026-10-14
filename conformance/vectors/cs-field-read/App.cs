using Microsoft.Extensions.Logging;

class Signup
{
    void Welcome(ILogger log, Account user)
    {
        log.LogInformation("user {Email}", user.Email);
    }
}
