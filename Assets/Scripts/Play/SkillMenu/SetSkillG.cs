using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
///using Photon;

public class SetSkillG : MonoBehaviour
{
    //public CatcherKeys ck;
    public Toggle G1;
    public Image IconG;
    GameObject Soldier;

    public void SetG()
    {
        GetComponent<SkillsLink>().KeyGSkill = null;
        if (G1.isOn)
        {
            GetComponent<SkillsLink>().KeyGSkill = SkillCode.TestSkill01;
            return;
        }
    }
}
