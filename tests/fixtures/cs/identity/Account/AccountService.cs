using Microsoft.AspNetCore.Identity;

namespace Shop.Account;

public class AppUser : IdentityUser
{
    public string CardNumber { get; set; } = "";
}

public record LoginModel(string Username, string Password);

public class AccountService(
    SignInManager<AppUser> signIn,
    UserManager<AppUser> users,
    IPasswordHasher<AppUser> hasher,
    ILogger<AccountService> logger)
{
    // Signing in hands the password to Identity, which stays inside the system.
    public async Task<bool> Login(LoginModel model)
    {
        var result = await signIn.PasswordSignInAsync(model.Username, model.Password, true, false);
        logger.LogInformation("login {Succeeded}", result.Succeeded);
        return result.Succeeded;
    }

    public async Task Register(LoginModel model)
    {
        var user = new AppUser { UserName = model.Username };
        user.PasswordHash = hasher.HashPassword(user, model.Password);
        await users.CreateAsync(user);
        logger.LogInformation("created {User}", user.UserName); // expect: PF001
    }

    public async Task Lookup(string id)
    {
        var user = await users.FindByIdAsync(id);
        logger.LogInformation("card {Card}", user!.CardNumber); // expect: PF001
    }
}
