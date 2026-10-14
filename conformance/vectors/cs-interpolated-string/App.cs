using System;

class Sms
{
    void Send(Account account)
    {
        Console.WriteLine($"texting {account.PhoneNumber} now");
    }
}
