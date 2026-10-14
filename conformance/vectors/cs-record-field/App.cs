using System;

record Entry(string Label, string Note);

class Directory
{
    void Show(Account account)
    {
        var entry = new Entry("anonymous", account.Email);
        Console.WriteLine(entry.Note);
        Console.WriteLine(entry.Label);
    }
}
