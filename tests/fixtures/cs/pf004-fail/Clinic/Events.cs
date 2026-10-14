using Mixpanel;

namespace Shop.Clinic;

public class Events(MixpanelClient mp)
{
    public Task Admitted(Patient patient) =>
        mp.TrackAsync("admitted", patient.Id, new { faith = patient.Religion }); // expect: PF004
}
