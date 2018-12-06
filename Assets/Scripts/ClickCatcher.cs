using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using System;
using System.IO;
using System.Text;

public class ClickCatcher : MonoBehaviour {
    public Toggle TotalSwitch;
    public string FileName;
    public string txtPath;
    FileStream fs;
    StreamWriter SWriter;
    public static List<ClickData> LS = new List<ClickData>();
    public LSWriter lsw;
    public NetWriter theNW;
    public Image SignalLight;

    void Update () {
        if (!TotalSwitch.isOn)
            return;
        if (Input.GetMouseButtonDown(0))
        {
            Vector2 mp = Camera.main.ScreenToWorldPoint(Input.mousePosition);
            ClickData cd = new ClickData();
            cd.setdata(MButton.left, mp.x, mp.y);
            theNW.L2S.Add(cd);
            LS.Add(cd);
            //SWriter.WriteLine(cd.ToP());
        }
        if (Input.GetMouseButtonDown(1))
        {
            Vector2 mp = Camera.main.ScreenToWorldPoint(Input.mousePosition);
            ClickData cd = new ClickData();
            cd.setdata(MButton.right, mp.x, mp.y);
            theNW.L2S.Add(cd);
            LS.Add(cd);
            //SWriter.WriteLine(cd.ToP());
        }
    }

    public void SelectNewTXT()
    {
        theNW.enabled = false;//
        if (SWriter != null)
            SWriter.Close();
        if (!TotalSwitch.isOn)
        {
            lsw.enabled = false;
            return;
        }
        if (SignalLight.color == Color.green)
            theNW.enabled = true;
        LS = new List<ClickData>();
        FileName = "/" + string.Format("{0:D2}{1:D2}{2:D2}{3:D2}{4:D2}{5:D2}", System.DateTime.Now.Year, System.DateTime.Now.Month, System.DateTime.Now.Day, System.DateTime.Now.Hour, System.DateTime.Now.Minute, System.DateTime.Now.Second) + ".txt";
        txtPath = Application.dataPath + FileName;
        fs = new FileStream(txtPath, FileMode.OpenOrCreate,FileAccess.ReadWrite);
        SWriter = new StreamWriter(fs);
        SWriter.WriteLine("Started");
        lsw.enabled = true;
    }
}
