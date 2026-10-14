using System;

class Service
{
    protected void Trace(string text) => Console.WriteLine(text);
}

class Signup : Service
{
    void Welcome(Account account) => Trace(account.PhoneNumber);
}
