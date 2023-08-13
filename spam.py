import multiprocessing as mp
import requests
import json

def get_block_number():
    payload = {
        "jsonrpc": "2.0",
        "method": "eth_getBlockByNumber",
        "params": ["0x1113924", False],
        "id": 1,
    }
    response = requests.post("http://127.0.0.1:3000", json=payload)
    result = response.json()
    block_number = int(result["result"]["number"], 16)
    # block_number = result
    return block_number

def my_func(x):
    for i in range(x): 
        block_number = get_block_number()
        print(f"Block Number: {block_number}")

def main():
    pool = mp.Pool(mp.cpu_count())
    pool.map(my_func, range(0, 1000))

if __name__ == "__main__":
    main()
