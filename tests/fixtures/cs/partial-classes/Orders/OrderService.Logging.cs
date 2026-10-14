namespace Shop.Orders;

public partial class OrderService
{
    public void Report() => logger.LogInformation("last order by {Email}", lastEmail); // expect: PF001
}
