using Acme.Sync;

namespace App.Profiles;

public class Profile
{
    public string Email { get; set; } = "";
    public string Name { get; set; } = "";
}

public class ProfilePage
{
    public async Task Save(Profile profile)
    {
        // Device storage: sinks, not gaps.
        await SecureStorage.SetAsync("email", profile.Email); // expect-flow
        Preferences.Set("email", profile.Email); // expect-flow
        // A call the catalogue does not know, with personal data: high impact.
        await Uploader.Upload(profile.Email); // expect: PFC01
        // Only a value whose category needs review: medium.
        Uploader.Tag(profile.Name); // expect: PFC01
        // A dialog: low impact, reported at info.
        await Shell.Current.DisplayAlert("Saved", profile.Email, "OK"); // expect: PFC01
    }
}
