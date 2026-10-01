from anthropic import Anthropic

client = Anthropic()


def draft_reply(customer, question):
    prompt = f"A {customer.tier} customer asks: {question}"
    return client.messages.create(
        model="claude",
        max_tokens=500,
        messages=[{"role": "user", "content": prompt}],
    )
