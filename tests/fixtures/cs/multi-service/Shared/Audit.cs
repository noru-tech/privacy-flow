namespace Shop.Shared;

// Compiled into Orders.Api and Web (`<Compile Include>`), a library of neither.
public static class Audit
{
    public static void Note(ILogger log, string email) =>
        log.LogInformation("audit {Email}", email); // expect: PF001
}
