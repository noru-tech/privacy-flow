using System;
using System.Collections.Generic;
using System.Linq;

class Digest
{
    void Send(List<Account> accounts)
    {
        var emails = accounts.Select(a => a.Email).ToList();
        Console.WriteLine(string.Join(", ", emails));
        Console.WriteLine(accounts.Count(a => a.Email.Length > 0));
    }
}
