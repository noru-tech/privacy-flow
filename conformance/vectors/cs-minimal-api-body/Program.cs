using System;
using Microsoft.AspNetCore.Builder;

var app = WebApplication.Create(args);
app.MapPost("/signup", (SignupForm form) => Console.WriteLine(form));
app.Run();
