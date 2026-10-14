using System.Net.Mail;
using Azure.Messaging.ServiceBus;
using RabbitMQ.Client;
using SendGrid;
using SendGrid.Helpers.Mail;

namespace Shop.Mail;

public class User
{
    public string Email { get; set; } = "";
    public string PhoneNumber { get; set; } = "";
}

public class Notifications(IChannel channel, ServiceBusSender sender, ISendGridClient sendGrid)
{
    public async Task Queue(User user)
    {
        var body = System.Text.Encoding.UTF8.GetBytes(user.Email);
        await channel.BasicPublishAsync("signups", "new", body); // expect-flow
    }

    public Task Bus(User user) =>
        sender.SendMessageAsync(new ServiceBusMessage(user.PhoneNumber)); // expect: PF002

    public Task Smtp(User user)
    {
        var message = new MailMessage("noreply@example.com", user.Email, "Welcome", "Hello");
        using var smtp = new SmtpClient("localhost");
        return smtp.SendMailAsync(message); // expect-flow
    }

    public Task Grid(User user)
    {
        var msg = MailHelper.CreateSingleEmail(new EmailAddress("app@example.com"), new EmailAddress(user.Email), "Hi", "Hi", "<p>Hi</p>");
        return sendGrid.SendEmailAsync(msg); // expect: PF002
    }
}
