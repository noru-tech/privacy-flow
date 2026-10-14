using Microsoft.AspNetCore.Mvc;

namespace Shop.Controllers;

public record ProfileUpdate(string Email, string Nickname);

[ApiController]
[Route("api/profile")]
public class ProfileController(ILogger<ProfileController> logger) : ControllerBase
{
    [HttpPut]
    public IActionResult Update([FromBody] ProfileUpdate update, [FromQuery] string source)
    {
        logger.LogInformation("update {Email} via {Source}", update.Email, source); // expect: PF001
        var agent = Request.Headers["User-Agent"];
        logger.LogInformation("agent {Agent}", agent); // expect: PF001
        return Ok();
    }

    // Not an action: its parameter is not request input.
    [NonAction]
    public void Audit(string note) => logger.LogInformation("audit {Note}", note);

    public static void Helper(string note) => Console.WriteLine(note);
}
