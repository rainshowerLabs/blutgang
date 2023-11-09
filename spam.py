import multiprocessing as mp
import requests
import json
import random

def get_block_number(hex_num):
    random_id = random.randint(1, 1000000)
    payload = {
        "jsonrpc": "2.0",
        "method": "eth_getBlockByNumber",
        "params": [hex_num, False],
        "id": random_id,
    }
    response = requests.post("http://127.0.0.1:3000", json=payload)
    result = response.json()
    try:
        block_number = int(result["result"]["number"], 16)
    except Exception as e:
        return e
    # block_number = result

    # throw if id doesnt match
    if result["id"] != random_id:
        raise ValueError("Invalid response id: %s" % result["id"])

    return block_number

def my_func(x):
    for i in range(x): 
        block_number = get_block_number(hex(i+17975056))
        # print(f"Block Number: {block_number}")

def main():
    pool = mp.Pool(mp.cpu_count())
    i = 0
    while i < 10:
        pool.map(my_func, range(0, 1000))
        print("done: ", i)
        i += 1

if __name__ == "__main__":
    main()
