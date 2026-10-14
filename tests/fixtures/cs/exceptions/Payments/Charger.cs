namespace Shop.Payments;

public class PaymentException(string message) : Exception(message);

public class Charger(ILogger<Charger> logger)
{
    public void Charge(string cardNumber)
    {
        try
        {
            Validate(cardNumber);
        }
        catch (PaymentException ex)
        {
            logger.LogError(ex, "charge failed: {Message}", ex.Message); // expect: PF001
        }
    }

    private static void Validate(string card)
    {
        if (card.Length != 16)
        {
            throw new PaymentException($"invalid card {card}");
        }
    }
}
