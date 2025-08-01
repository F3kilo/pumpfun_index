var chart = anychart.stock();
chart.container("myChart");
var dataTable = anychart.data.table("x");
var mapping = dataTable.mapAs({ open: "open", high: "high", low: "low", close: "close" });
var ohlcSeries = chart.plot(0).ohlc(mapping);

var socket;

fetch("http://localhost:33987/tokens")
  .then((response) => {
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    return response.json();
  })
  .then((data) => {
    for (var i = 0; i < data.length; i++) {
      let li = document.createElement('li');
      li.textContent = data[i];
      li.onclick = function () {
        drawChart(this.textContent);
      }
      document.getElementById("tokens").appendChild(li);
    }
  })
  .catch((error) => console.error("Fetch error:", error));

function drawChart(token) {
  if (socket != null) {
    socket.close();
  }

  console.log(token);
  document.getElementById("myChart").hidden = false;

  // const data = [
  //   { "x": "2015-12-25", "open": 512.53, "high": 514.88, "low": 505.69, "close": 507.34 },
  //   { "x": "2015-12-26", "open": 511.83, "high": 514.98, "low": 505.59, "close": 506.23 },
  //   { "x": "2015-12-27", "open": 511.22, "high": 515.30, "low": 505.49, "close": 506.47 },
  //   { "x": "2015-12-28", "open": 510.35, "high": 515.72, "low": 505.23, "close": 505.80 },
  //   { "x": "2015-12-29", "open": 510.53, "high": 515.86, "low": 505.38, "close": 508.25 },
  //   { "x": "2015-12-30", "open": 511.43, "high": 515.98, "low": 505.66, "close": 507.45 },
  //   { "x": "2015-12-31", "open": 511.50, "high": 515.33, "low": 505.99, "close": 507.98 },
  //   { "x": "2016-01-01", "open": 511.32, "high": 514.29, "low": 505.99, "close": 506.37 },
  //   { "x": "2016-01-02", "open": 511.70, "high": 514.87, "low": 506.18, "close": 506.75 }
  // ];
  // dataTable.addData(data);

  ohlcSeries.name(token);
  chart.title(token + " chart");

  socket = new WebSocket("ws://localhost:33987/chart_data_ws/" + token);

  dataTable.remove();

  socket.onmessage = function (event) {
    var data = JSON.parse(event.data);

    var candle = data.candle;
    const date = new Date(data.timestamp * 1000);
    candle.x = date;
    dataTable.addData([data.candle]);

    chart.draw();
  };
}
