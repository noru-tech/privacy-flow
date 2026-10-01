from anthropic import Anthropic

client = Anthropic()


def draft_reply(customer, question):
    prompt = f"Customer {customer.email} asks: {question}"
    return client.messages.create(  # expect: PF002 PF003
        model="claude",
        max_tokens=500,
        messages=[{"role": "user", "content": prompt}],
    )
