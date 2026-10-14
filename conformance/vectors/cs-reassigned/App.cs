using System;

class Signup
{
    void Welcome(Account account)
    {
        var text = account.Email;
        text = "hidden";
        Console.WriteLine(text);
    }
}
