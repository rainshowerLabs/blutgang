import threading
import time
import websocket
import json
import uuid

# Set the WebSocket server URL
websocket_url = "ws://127.0.0.1:3000"

# Number of clients to simulate
num_clients = 100

# Number of messages to send per client
num_messages = 1000

# Interval to display requests per second (in seconds)
display_interval = 5

def generate_random_id():
    return random.randint(1, 100000)

def websocket_client(client_id):
    print(f"Client {client_id} started.")
    ws = websocket.WebSocket()
    ws.connect(websocket_url)

    for i in range(num_messages):
        json_rpc_message = {
            "jsonrpc": "2.0",
            "id": generate_random_id(),
            "method": "eth_gasPrice"
        }
        message = json.dumps(json_rpc_message)
        ws.send(message)
        print(f"Client {client_id} sent: {message}")

    ws.close()
    print(f"Client {client_id} finished.")

def run_stress_test():
    threads = []

    for i in range(num_clients):
        thread = threading.Thread(target=websocket_client, args=(i,))
        threads.append(thread)
        thread.start()

    for thread in threads:
        thread.join()

def display_requests_per_second(start_time, total_requests):
    current_time = time.time()
    elapsed_time = current_time - start_time
    rps = total_requests / elapsed_time if elapsed_time > 0 else 0
    print(f"Requests per second: {rps:.2f}")

if __name__ == "__main__":
    total_requests = 0
    start_time = time.time()

    while True:
        run_stress_test()
        total_requests += num_clients * num_messages
        display_requests_per_second(start_time, total_requests)
        time.sleep(display_interval)
