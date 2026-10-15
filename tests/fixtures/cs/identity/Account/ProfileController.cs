using System.Security.Claims;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Mvc;
using Microsoft.Extensions.AI;

namespace Shop.Account;

// A claims list holds the subject, name, email and whatever else the identity provider asserted,
// together: personal data of category `user`, not data of undetermined category.
public class ProfileController(IChatClient chat, ILogger<ProfileController> logger) : Controller
{
    [HttpGet]
    public async Task<IActionResult> Callback()
    {
        var result = await HttpContext.AuthenticateAsync("External");
        var claims = result.Principal!.Claims.Select(c => $"{c.Type}: {c.Value}");
        logger.LogDebug("External claims: {@Claims}", claims); // expect: PF001
        return Ok();
    }

    [HttpGet]
    public async Task<string> Greeting()
    {
        var who = string.Join(", ", User.Claims.Select(c => c.Value));
        var reply = await chat.GetResponseAsync($"Greet this customer: {who}"); // expect: PF003
        return reply.Text;
    }
}

public class ChatState(ClaimsPrincipal user, IChatClient chat)
{
    public async Task<string> Ask()
    {
        var prompt = $"Claims: {string.Join(", ", user.Claims)}";
        var reply = await chat.GetResponseAsync(prompt); // expect: PF003
        return reply.Text;
    }

    // A claim read off the list into a classified field is reported as that field: an address,
    // not the whole list.
    public void LogAddress(ILogger<ChatState> logger)
    {
        var address = new ClaimedAddress(user.Claims.FirstOrDefault(c => c.Type == "address_city")?.Value);
        logger.LogInformation("city {City}", address.City); // expect: PF001
    }
}

public record ClaimedAddress(string? City);
