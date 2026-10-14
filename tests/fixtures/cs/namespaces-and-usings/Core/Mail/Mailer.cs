namespace Shop.Core.Mail
{
    public class Mailer
    {
        public void Deliver(string to, string body) => System.Console.WriteLine($"to {to}: {body}"); // expect: PF001
    }
}
