namespace Shop.Web.Services;

// Item, brand and type IDs identify things, not people: no finding. The search text is request
// input of undetermined category: an uncertain finding, at info.
public class CatalogService(HttpClient http, IConfiguration config)
{
    public Task<string> GetItem(int itemId) =>
        http.GetStringAsync($"{config["CatalogUrl"]}/items/{itemId}");

    public Task<string> Find(int[]? brandIds, int? typeId) =>
        http.GetStringAsync($"{config["CatalogUrl"]}/items?brand={brandIds}&type={typeId}");

    public Task<string> Search(string? text) =>
        http.GetStringAsync($"{config["CatalogUrl"]}/items?q={text}"); // expect: PF006
}
