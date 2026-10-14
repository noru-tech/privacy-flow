using Microsoft.AspNetCore.Identity;

namespace Shop.Auth;

public class User
{
    public string Id { get; set; } = "";
}

public class Registration(IPasswordHasher<User> hasher, ILogger<Registration> logger)
{
    public void Register(User user, string password)
    {
        var hashed = hasher.HashPassword(user, password);
        logger.LogInformation("stored hash of length {Length} for {Id}", hashed.Length, user.Id);
    }
}
