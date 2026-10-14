using Grpc.Core;

namespace Shop.Grpc;

// `Greeter.GreeterBase`, `HelloRequest` and `HelloReply` are generated from a .proto file at
// build time; they are not in the repository.
public class GreeterService(ILogger<GreeterService> logger) : Greeter.GreeterBase
{
    public override Task<HelloReply> SayHello(HelloRequest request, ServerCallContext context)
    {
        logger.LogInformation("hello {Request}", request); // expect: PF001
        var user = context.GetHttpContext().User.FindFirst("sub")?.Value;
        logger.LogInformation("by {User}", user);
        return Task.FromResult(new HelloReply());
    }
}
