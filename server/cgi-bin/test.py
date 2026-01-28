import sys
import json

path = sys.argv[0]
method = sys.argv[1]
body = sys.argv[2]

match method:
    case "GET":
        print(json.dumps({"body": "<p>GET</p>"}))
    
    case "POST":
        print(json.dumps({"body": "<p>POST</p>"}))
    
    case "DELETE":
        print(json.dumps({"body": "<p>DELETE</p>"}))