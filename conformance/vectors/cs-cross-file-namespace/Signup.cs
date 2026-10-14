using App.Util;

namespace App;

class Signup
{
    void Welcome(Account account)
    {
        Audit.Note(account.Email);
    }
}
