import torch
from transformers import GPT2LMHeadModel, GPT2Tokenizer

class CustomerServiceGPT2:
    def __init__(self, model_name='gpt2'):
        """Initialize the dialogue model
        
        Args:
            model_name: Name of pretrained model, default is 'gpt2'
        """
        self.tokenizer = GPT2Tokenizer.from_pretrained(model_name)
        self.model = GPT2LMHeadModel.from_pretrained(model_name)
        self.device = torch.device('cuda' if torch.cuda.is_available() else 'cpu')
        self.model.to(self.device)
        
        # Set special tokens
        self.tokenizer.pad_token = self.tokenizer.eos_token
        self.tokenizer.sep_token = '<|sep|>'
        
    def generate_response(self, task_desc, context, max_length=100):
        """Generate dialogue response
        
        Args:
            task_desc: Task description
            context: Dialogue context
            max_length: Maximum length of generated text
            
        Returns:
            Generated response text
        """
        # Build input text
        input_text = f"Task:{task_desc}\nConversation History:{context}\nResponse:"
        
        # Encode input
        inputs = self.tokenizer.encode(input_text, return_tensors='pt')
        inputs = inputs.to(self.device)
        
        # Generate response
        outputs = self.model.generate(
            inputs,
            max_length=max_length + len(inputs[0]),
            num_return_sequences=1,
            no_repeat_ngram_size=2,
            temperature=0.7,
            top_k=50,
            top_p=0.9,
            pad_token_id=self.tokenizer.pad_token_id,
            eos_token_id=self.tokenizer.eos_token_id
        )
        
        # Decode output
        response = self.tokenizer.decode(outputs[0], skip_special_tokens=True)
        response = response[len(input_text):].strip()
        
        return response
    
    def chat(self, task_desc):
        """Conduct multi-turn dialogue
        
        Args:
            task_desc: Task description
        """
        context = []
        print(f"Task: {task_desc}")
        print("Start conversation (type 'quit' to end):")
        
        while True:
            user_input = input("Customer: ")
            if user_input.lower() == 'quit':
                break
                
            # Update conversation history
            context.append(f"Customer: {user_input}")
            context_str = "\n".join(context[-3:])  # Keep last 3 turns
            
            # Generate response
            response = self.generate_response(task_desc, context_str)
            print(f"Agent: {response}")
            
            # Update conversation history
            context.append(f"Agent: {response}")

# Usage example
if __name__ == "__main__":
    chatbot = CustomerServiceGPT2()
    task = """You are a professional customer service agent. Your task is to:
    - Greet customers politely
    - Answer product-related questions professionally
    - Help resolve customer issues
    - Maintain a friendly and helpful tone
    - Follow up to ensure customer satisfaction"""
    chatbot.chat(task)

