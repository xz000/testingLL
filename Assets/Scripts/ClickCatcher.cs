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
    public static List<string> LS;
    public LSWriter lsw;

    void Update () {
        if (!TotalSwitch.isOn)
            return;
        if (Input.GetMouseButtonDown(0))
        {
            Vector2 mp = Camera.main.ScreenToWorldPoint(Input.mousePosition);
            string m0st = "Mouse0:" + mp.x + "," + mp.y;
            LS.Add(m0st);
            NetWriter.L2S.Add(m0st);
            SWriter.WriteLine(m0st);
        }
        if (Input.GetMouseButtonDown(1))
        {
            Vector2 mp = Camera.main.ScreenToWorldPoint(Input.mousePosition);
            string m1st = "Mouse1:" + mp.x + "," + mp.y;
            LS.Add(m1st);
            NetWriter.L2S.Add(m1st);
            SWriter.WriteLine(m1st);
        }
    }

    public void SelectNewTXT()
    {
        if (SWriter != null)
            SWriter.Close();
        if (!TotalSwitch.isOn)
        {
            lsw.enabled = false;
            return;
        }
        LS = new List<string>();
        FileName = "/" + string.Format("{0:D2}{1:D2}{2:D2}{3:D2}{4:D2}{5:D2}", System.DateTime.Now.Year, System.DateTime.Now.Month, System.DateTime.Now.Day, System.DateTime.Now.Hour, System.DateTime.Now.Minute, System.DateTime.Now.Second) + ".txt";
        txtPath = Application.dataPath + FileName;
        fs = new FileStream(txtPath, FileMode.OpenOrCreate,FileAccess.ReadWrite);
        SWriter = new StreamWriter(fs);
        SWriter.WriteLine("Started");
        lsw.enabled = true;
    }
}
