using System;

class Lookup
{
    static bool TryFind(Account account, out string note)
    {
        note = account.Email;
        return true;
    }

    void Show(Account account)
    {
        var found = TryFind(account, out var note);
        Console.WriteLine(found);
    }
}
