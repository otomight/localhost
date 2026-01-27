import sys

path = sys.argv[0]
method = sys.argv[1]
body = sys.argv[2]

match method:
    case "GET":
        print("<p>GET</p>")
    
    case "POST":
        print("<p>POST</p>")
    
    case "DELETE":
        print("<p>DELETE</p>")