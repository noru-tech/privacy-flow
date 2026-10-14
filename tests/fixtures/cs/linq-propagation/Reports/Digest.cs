namespace Shop.Reports;

public class Customer
{
    public string Email { get; set; } = "";
    public string Tier { get; set; } = "";
}

public class Digest(ILogger<Digest> logger)
{
    public void Send(List<Customer> customers)
    {
        var emails = customers.Where(c => c.Tier == "gold").Select(c => c.Email).ToList();
        logger.LogInformation("gold: {Emails}", string.Join(", ", emails)); // expect: PF001

        var first = customers.FirstOrDefault()?.Email ?? "";
        var builder = new System.Text.StringBuilder();
        builder.Append("first: ").Append(first);
        logger.LogInformation(builder.ToString()); // expect: PF001

        var query = from c in customers where c.Tier != "" select c.Email;
        logger.LogInformation("all: {All}", query); // expect: PF001

        // Counts and masks carry none of the data.
        logger.LogInformation("count {Count} {Length}", customers.Count, first.Length);
        logger.LogInformation("has gold {Any}", customers.Any(c => c.Email.EndsWith("@gold.example")));
    }
}
