using System;

class Box
{
    public string Content { get; }

    public Box(string content)
    {
        Content = content;
    }
}

class Signup
{
    void Welcome(Account account)
    {
        var secret = new Box(account.Email);
        var plain = new Box("nothing");
        Console.WriteLine(plain.Content);
    }
}
