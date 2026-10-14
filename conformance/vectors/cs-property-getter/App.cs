using System;

class Card
{
    public string Holder { get; set; } = "";
    public string Label => "card of " + Holder;
}

class Wallet
{
    void Show(Account account)
    {
        var card = new Card { Holder = account.PhoneNumber };
        Console.WriteLine(card.Label);
    }
}
