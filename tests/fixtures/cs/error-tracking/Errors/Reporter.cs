using Microsoft.ApplicationInsights;
using Sentry;

namespace Shop.Errors;

public class Customer
{
    public string Email { get; set; } = "";
}

public class Reporter(TelemetryClient telemetry)
{
    public void Failed(Customer customer, Exception error)
    {
        SentrySdk.ConfigureScope(scope => scope.SetTag("email", customer.Email)); // expect: PF002
        SentrySdk.CaptureMessage($"checkout failed for {customer.Email}"); // expect: PF002
        telemetry.TrackEvent("checkout-failed", new Dictionary<string, string> { ["email"] = customer.Email }); // expect: PF002
    }
}
