using Microsoft.AspNetCore.SignalR;

namespace Shop.Hubs;

public class ChatHub : Hub // expect: PFC01
{
    public Task Send(string message) => Clients.All.SendAsync("message", message);
}
