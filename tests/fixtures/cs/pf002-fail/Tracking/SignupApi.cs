using Segment;

namespace Shop.Tracking;

public record SignupRequest(string UserId, string Email);

public static class SignupApi
{
    public static void MapSignup(this IEndpointRouteBuilder app)
    {
        app.MapPost("/signup", (SignupRequest req) =>
        {
            Analytics.Client.Identify(req.UserId, new Dictionary<string, object> { ["email"] = req.Email }); // expect: PF002
            return Results.Ok();
        });
    }
}
