import sys
import json
import sqlite3
import hashlib
import uuid

con = sqlite3.connect("server/bdd.sqlite")
cur = con.cursor()
path = sys.argv[0]
method = sys.argv[1]
body = sys.argv[2]
cookie = sys.argv[3]

def checkSession(cookie: str):
    res = cur.execute("SELECT * FROM session WHERE uuid = ?", (cookie,)).fetchone()
    return res is not None

match method:
    case "GET":
        if (checkSession(cookie)):
            response = {
                "headers": {
                    "Location": "index.py"
                },
                "status": 303,
                "body": "OK",
            }
            print(json.dumps(response))
        else:
            with open("server/templates/login.html", "r") as file :
                content = file.read()
                result = {
                    "body": content
                }
                print(json.dumps(result))
                file.close()

    case "POST":
        parsed_body = {}
        for val in body.split("\r\n"):
            if len(val) > 0:
                entry = val.split("=", 1)
                parsed_body[entry[0]] = entry[1]

        # requete bdd
        res = cur.execute("SELECT * FROM users WHERE email = ? AND password = ?", (
            parsed_body["email"],
            hashlib.sha256(str(parsed_body["password"]).encode('utf-8')).hexdigest())
            ).fetchall()

        if len(res) > 0 :
            token = f'{uuid.uuid4()}'

            cur.execute("INSERT INTO session(user_id, uuid) VALUES (?, ?)", (
                res[0][0],
                token
            ))
            con.commit()

            # print resultat
            result = {
                "headers": {
                    "Set-Cookie": "session=" + token,
                    "Location": "index.py"
                },
                "status": 303,
                "body": "OK",
            }
            print(json.dumps(result))

        else:
            print(json.dumps({
                "error":[500, "Something went wrong"],
                "body": "NOK"
            }))
