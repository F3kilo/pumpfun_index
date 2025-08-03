var chart = anychart.stock();
chart.container("myChart");
var dataTable = anychart.data.table("x");
var mapping = dataTable.mapAs({ open: "open", high: "high", low: "low", close: "close" });
var ohlcSeries = chart.plot(0).ohlc(mapping);

var socket;
var token;
var tokenName;

const maxChartDataLen = 100;

var resolutionSelector = document.getElementById("resolution-select");
resolutionSelector.onchange = function () {
  drawChart();
}

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
      var text = data[i][0];

      if (data[i][1].symbol != 'NAN') {
        text = data[i][1].symbol;
      }

      if (data[i][1].name != 'unknown') {
        text = text + " | " + data[i][1].name;
      }

      li.textContent = text;
      li.id = data[i][0];

      li.onclick = function () {
        token = this.id;
        tokenName = this.textContent;
        drawChart();
      }
      document.getElementById("tokens").appendChild(li);
    }
  })
  .catch((error) => console.error("Fetch error:", error));

function drawChart() {
  if (socket != null) {
    socket.close();
  }

  console.log(token);

  document.getElementById("myChart").hidden = false;

  ohlcSeries.name(token);
  chart.title(tokenName + " | " + token);

  var resolutionSelector = document.getElementById("resolution-select");
  const resolution = resolutionSelector.options[resolutionSelector.selectedIndex].value;
  socket = new WebSocket("ws://localhost:33987/chart_data_ws/" + token + "/" + resolution);

  dataTable.remove();

  socket.onmessage = function (event) {
    var data = JSON.parse(event.data);

    selectable = mapping.createSelectable();
    selectable.selectAll();

    var iterator = selectable.getIterator();
    var items_count = 0;


    while (iterator.advance()) {
      items_count++;
    }

    if (items_count > maxChartDataLen) {
      dataTable.removeFirst(items_count - maxChartDataLen);
    }

    var candle = data.candle;
    const date = new Date(data.timestamp * 1000);
    candle.x = date;

    dataTable.addData([candle]);


    chart.draw();
  };
}
