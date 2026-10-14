using Acme.Mail;

namespace Shop.Mail;

public class User
{
    public string Email { get; set; } = "";
}

public class Welcome
{
    public void Send(User user) => Mailer.Deliver(user.Email, "Welcome!"); // expect: PFC01
}
