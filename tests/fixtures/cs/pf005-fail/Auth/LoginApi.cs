using Serilog;

namespace Shop.Auth;

public record LoginRequest(string Email, string Password);

public static class LoginApi
{
    public static void MapLogin(this WebApplication app)
    {
        app.MapPost("/login", (LoginRequest req) =>
        {
            Log.Information("login attempt with {Password}", req.Password); // expect: PF005
            return Results.Ok();
        });
    }
}
