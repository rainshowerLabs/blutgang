import multiprocessing as mp
import requests

def my_func(x):
    for i in range(x): 
        print(requests.get("http://127.0.0.1:3000"))

def main():
    pool = mp.Pool(mp.cpu_count())
    pool.map(my_func, range(0, 1000))
if __name__ == "__main__":

    main()
