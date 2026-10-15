using System.ComponentModel.DataAnnotations.Schema;
using Microsoft.EntityFrameworkCore;
using Microsoft.EntityFrameworkCore.Metadata.Builders;

namespace Shop.Orders.Data;

public class Address
{
    public string Street { get; set; } = "";
    public string City { get; set; } = "";
}

public class Buyer
{
    public int Id { get; set; }
    public string Email { get; set; } = "";
    [Column("phone")]
    public string PhoneNumber { get; set; } = "";
    [NotMapped]
    public string Nickname { get; set; } = "";
    public List<Order> Orders { get; set; } = new();
}

public class Order
{
    private string _cardNumber = "";
    public int Id { get; set; }
    public Address ShipTo { get; set; } = new();
    public Buyer? Buyer { get; set; }
    public void Pay(string cardNumber) => _cardNumber = cardNumber;
}

class OrderConfiguration : IEntityTypeConfiguration<Order>
{
    public void Configure(EntityTypeBuilder<Order> builder)
    {
        builder.ToTable("orders");
        builder.OwnsOne(o => o.ShipTo);
        builder.Property("_cardNumber").HasColumnName("CardNumber");
        builder.Property<int>("BuyerId");
    }
}

public class OrdersContext(DbContextOptions<OrdersContext> options) : DbContext(options)
{
    public DbSet<Buyer> Buyers { get; set; }
    public DbSet<Order> Orders { get; set; }

    protected override void OnModelCreating(ModelBuilder modelBuilder)
    {
        modelBuilder.HasDefaultSchema("sales");
        modelBuilder.ApplyConfiguration(new OrderConfiguration());
    }
}
