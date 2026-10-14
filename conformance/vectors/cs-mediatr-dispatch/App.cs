using System;
using System.Threading;
using System.Threading.Tasks;
using MediatR;

record Register(string Note) : IRequest<bool>;

class LoggingBehavior<TRequest, TResponse> : IPipelineBehavior<TRequest, TResponse> where TRequest : notnull
{
    public async Task<TResponse> Handle(TRequest request, RequestHandlerDelegate<TResponse> next, CancellationToken ct)
    {
        Console.WriteLine(request);
        return await next();
    }
}

class Endpoint(ISender sender)
{
    Task<bool> Post(Account account) => sender.Send(new Register(account.Email));
}
