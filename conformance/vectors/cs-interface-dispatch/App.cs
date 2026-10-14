using System;

interface INotifier
{
    void Notify(string to);
}

class ConsoleNotifier : INotifier
{
    public void Notify(string to) => Console.WriteLine(to);
}

class Signup(INotifier notifier)
{
    void Welcome(Account account) => notifier.Notify(account.Email);
}
