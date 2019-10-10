import socket
from math import floor
import struct
from adafruit_rplidar import RPLidar
import ctypes

PORT_NAME = '/dev/ttyUSB0'
lidar = RPLidar(None, PORT_NAME)
lidar.connect()
lidar.set_pwm(1023)
lidar.start_motor()

data_socket = socket.socket()
data_socket.connect(('192.168.1.204', 8002))
connection = data_socket.makefile('wb')

scan_data = [0] * 360
buf = (ctypes.c_float * 360)()

try:
    for scan in lidar.iter_scans():
        for (_, angle, distance) in scan:
            scan_data[min([359, floor(angle)])] = distance
        buf[:] = scan_data
        connection.write(buf)
except:
    print("Stopping collection.")
lidar.stop()
lidar.disconnect()
