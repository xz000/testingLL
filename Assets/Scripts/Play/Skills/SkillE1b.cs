using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillE1b : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance;
    public float bulletspeed = 3;
    public GameObject fireball;
    //public float force = 15;
    //public float damage = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillE1b()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
        }
    }

    public void Skill(Fix64Vector2 actionplacef)
    {
        Vector2 actionplace = actionplacef.ToV2();
        GetComponent<DoSkill>().BeforeSkill();
        Vector2 singplace = transform.position;
        Vector2 skilldirection = actionplace - singplace;
        DoFire(singplace + 1.01f * skilldirection.normalized, skilldirection.normalized * bulletspeed);
        currentcooldown = 0;
        skillavaliable = false;
    }

    //[PunRPC]
    void DoFire(Vector2 fireplace, Vector2 speed2d)
    {
        GameObject bullet;
        //fireball.GetComponent<BombExplode>().sender = gameObject;
        bullet = Instantiate(fireball, fireplace, Quaternion.identity);
        //bullet.GetComponent<BombExplode>().bombpower = force;
        //bullet.GetComponent<BombExplode>().bombdamage = damage;
        bullet.GetComponent<RollScript>().v = speed2d;
        //bullet.GetComponent<BombExplode>().maxtime = maxdistance / bulletspeed;
    }

    void SkillE1bSetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }

    void LinkToIcon()
    {
        if (enabled)
            GameObject.Find("Canvas2").GetComponent<BottomLink>().iE.Fif = CalcFA;
    }
}
