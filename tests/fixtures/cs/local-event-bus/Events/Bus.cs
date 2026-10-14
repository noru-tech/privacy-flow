using RabbitMQ.Client;

namespace Shop.Events;

public abstract record IntegrationEvent;

public record UserRegistered(string Email) : IntegrationEvent;

public interface IEventBus
{
    Task PublishAsync(IntegrationEvent e);
}

// The project's own bus: an interface call reaches the RabbitMQ implementation.
public class RabbitMqBus(IChannel channel) : IEventBus
{
    public Task PublishAsync(IntegrationEvent e)
    {
        var body = System.Text.Json.JsonSerializer.SerializeToUtf8Bytes(e, e.GetType());
        return channel.BasicPublishAsync("events", "", body).AsTask(); // expect-flow
    }
}

public class Registration(IEventBus bus)
{
    public Task Register(string email) => bus.PublishAsync(new UserRegistered(email));
}
