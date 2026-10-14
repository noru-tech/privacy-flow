namespace Shop.Orders;

public partial class OrderService(ILogger<OrderService> logger)
{
    private string? lastEmail;

    public void Remember(string email) => lastEmail = email;
}
