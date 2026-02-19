import sys
import json
import sqlite3
# from ..utils import *

path = sys.argv[0]
method = sys.argv[1]
body = sys.argv[2]
cookie = sys.argv[3]
con = sqlite3.connect("server/bdd.sqlite")
cur = con.cursor()

def checkSession(cookie: str):
    res = cur.execute("SELECT * FROM session WHERE uuid = ?", (cookie,)).fetchone()
    return res is not None

match method:
    case "GET":
        if checkSession(cookie):
             with open("server/templates/images.html", "r") as file :
                content = file.read()
                result = {
                    "body": content
                }
                print(json.dumps(result))
                file.close()
        else:
            response = {
                "headers": {
                    "Location": "register.py"
                },
                "status": 303,
                "body": "OK",
            }
            print(json.dumps(response))
    
    case _:
        print(json.dumps([404, "NOK"]))
