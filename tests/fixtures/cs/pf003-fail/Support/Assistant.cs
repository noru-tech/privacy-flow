using Microsoft.Extensions.AI;
using OpenAI.Chat;

namespace Shop.Support;

public class Customer
{
    public string Email { get; set; } = "";
    public string Tier { get; set; } = "";
}

public class Assistant(IChatClient chat, ChatClient openai)
{
    public async Task<string> DraftReply(Customer customer, string question)
    {
        var prompt = $"Customer {customer.Email} asks: {question}";
        var reply = await chat.GetResponseAsync(prompt); // expect: PF003
        await openai.CompleteChatAsync(prompt); // expect: PF002 PF003
        return reply.Text;
    }
}
