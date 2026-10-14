using OpenAI.Chat;

namespace Shop.Support;

public class Customer
{
    public string Email { get; set; } = "";
    public string Tier { get; set; } = "";
}

public class Assistant(ChatClient openai)
{
    public async Task DraftReply(Customer customer, string question)
    {
        var prompt = $"A {customer.Tier} customer asks: {question}";
        await openai.CompleteChatAsync(prompt);
    }
}
