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
    case "GET" | "POST" :
        if checkSession(cookie):
             with open("server/templates/index.html", "r") as file :
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

    case "DELETE":
        cur.execute("DELETE FROM session WHERE uuid = ?", (cookie,))
        con.commit()
        response = {
            "headers": {
                "Set-Cookie": "session=deleted; path=/; expires=Thu, 01 Jan 1970 00:00:00 GMT",
                "Location": "register.py",
            },
            "status": 200,
            "body": "OK",
        }
        print(json.dumps(response))

