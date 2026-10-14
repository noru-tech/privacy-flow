namespace Shop.Accounts;

public static class Privacy
{
    public static string Mask(string value) => value[..1] + "***";
}
