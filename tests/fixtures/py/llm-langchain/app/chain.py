from langchain_openai import ChatOpenAI

llm = ChatOpenAI(model="gpt-4o")


def summarize(ticket):
    text = "\n".join([ticket.subject, ticket.email])
    return llm.invoke(text)  # expect: PF002 PF003
