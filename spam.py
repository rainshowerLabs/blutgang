import multiprocessing as mp
import requests
import json

def get_block_number(hex_num):
    payload = {
        "jsonrpc": "2.0",
        "method": "eth_getBlockByNumber",
        "params": [hex_num, False],
        "id": 1,
    }
    response = requests.post("http://127.0.0.1:3000", json=payload)
    result = response.json()
    try:
        block_number = int(result["result"]["number"], 16)
    except Exception as e:
        return e
    # block_number = result
    return block_number

def my_func(x):
    for i in range(x): 
        block_number = get_block_number(hex(i+17905056))
        print(f"Block Number: {block_number}")

def main():
    pool = mp.Pool(mp.cpu_count())
    while True:
        pool.map(my_func, range(0, 10000))

if __name__ == "__main__":
    main()
